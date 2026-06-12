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

/// Static product and pairing config supplied before the Rust core handles
/// product calls. One core instance represents one product identity.
///
/// `hostName`, `hostIcon`, `hostVersion`, `platformType`, and
/// `platformVersion` describe the host to the wallet during SSO pairing.
/// `peopleChainGenesisHash` must be exactly 32 bytes.
public struct RuntimeConfig: Sendable {
    public let productLabel: String
    public let productId: String
    public let siteId: String
    public let hostName: String
    public let hostIcon: String?
    public let hostVersion: String?
    public let platformType: String?
    public let platformVersion: String?
    public let peopleChainGenesisHash: Data
    public let pairingDeeplinkScheme: PairingDeeplinkScheme

    public init(
        productLabel: String,
        productId: String,
        siteId: String,
        hostName: String,
        hostIcon: String? = nil,
        hostVersion: String? = nil,
        platformType: String? = nil,
        platformVersion: String? = nil,
        peopleChainGenesisHash: Data,
        pairingDeeplinkScheme: PairingDeeplinkScheme = .polkadotApp
    ) {
        self.productLabel = productLabel
        self.productId = productId
        self.siteId = siteId
        self.hostName = hostName
        self.hostIcon = hostIcon
        self.hostVersion = hostVersion
        self.platformType = platformType
        self.platformVersion = platformVersion
        self.peopleChainGenesisHash = peopleChainGenesisHash
        self.pairingDeeplinkScheme = pairingDeeplinkScheme
    }

    fileprivate var native: NativeRuntimeConfig {
        NativeRuntimeConfig(
            productLabel: productLabel,
            productId: productId,
            siteId: siteId,
            hostName: hostName,
            hostIcon: hostIcon,
            hostVersion: hostVersion,
            platformType: platformType,
            platformVersion: platformVersion,
            peopleChainGenesisHash: peopleChainGenesisHash,
            pairingDeeplinkScheme: pairingDeeplinkScheme.native
        )
    }
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
        let safeUrl = jsStringLiteral(url)
        let safeToken = jsStringLiteral(token)
        return """
        (function() {
          window.__truapi_localhost = { url: \(safeUrl), token: \(safeToken) };
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

/// Storage backend the host provides to the Rust core. Throwing closures
/// can surface quota or unknown failures by raising `HostStorageError`
/// (defined in the generated bindings).
public protocol HostStorageBackend: AnyObject, Sendable {
    func read(key: String) throws -> Data?
    func write(key: String, value: Data) throws
    func clear(key: String) throws
}

/// Host-side callback bundle that the Rust core invokes for capabilities the
/// native shell owns. Rust owns the wire/protocol decoding and calls this
/// surface with native-friendly values.
///
/// Threading: the Rust core invokes every callback on a background thread it
/// owns, never the main thread. UI-decision callbacks
/// (``navigateTo(url:)``, ``devicePermission(capability:)``,
/// ``remotePermission(permission:domains:)``, ``paymentRequest(amountInPlanks:destination:)``,
/// ``paymentBalanceSubscribe()``, ``paymentTopUp(amountInPlanks:source:)``,
/// ``paymentStatusSubscribe(paymentId:)``, the `confirm*` family, and
/// ``submitPreimage(value:)``) each run on their own thread from a blocking
/// pool, so an implementation may safely block its calling thread (e.g. with
/// `DispatchQueue.main.sync` or a semaphore) until the user decides; other
/// TrUAPI traffic keeps flowing. The remaining callbacks (auth state,
/// storage, session, chain, feature, theme, preimage lookups) run inline on
/// the dispatcher thread and must return promptly without blocking.
/// Any UI work MUST still hop to the main thread, e.g.
/// `await MainActor.run { ... }` or `DispatchQueue.main.async { ... }`. Calling
/// UIKit/WebKit off the main thread is undefined behaviour.
public protocol HostBridge: AnyObject, Sendable {
    /// Lifecycle logger. Marker is a stable slug, detail is free-form.
    func onCoreLog(marker: String, detail: String)

    /// Open a URL in the system browser. Invoked on a blocking-pool thread;
    /// hop to the main thread to present UI. May block the calling thread if
    /// the user has to approve the navigation.
    func navigateTo(url: String) throws

    /// Deliver a push notification and return the host-assigned notification
    /// id. Invoked on the `truapi-ws-bridge` worker thread; hop to the main
    /// thread for any UI work.
    func pushNotification(text: String, deeplink: String?, scheduledAtMs: UInt64?) throws -> UInt32

    /// Cancel a previously scheduled notification id.
    func cancelNotification(id: UInt32) throws

    /// Prompt for a device-level permission. Returns the granted flag. Invoked
    /// on the `truapi-ws-bridge` worker thread; present the prompt on the main
    /// thread and block this thread until the user decides.
    func devicePermission(capability: String) throws -> Bool

    /// Prompt for a remote (product-scoped) permission bundle. Invoked on the
    /// `truapi-ws-bridge` worker thread; present the prompt on the main thread
    /// and block this thread until the user decides.
    func remotePermission(permission: String, domains: [String]) throws -> Bool

    /// Observe an auth state change. The core emits states only when they
    /// actually change, in transition order: render `.pairing` as the pairing
    /// QR UI, `.connected`/`.disconnected` as the account badge, and
    /// `.loginFailed` as a retryable error. Report a user dismissal of the
    /// pairing UI through ``TrUAPIHostCore/cancelLogin()``. Invoked on the
    /// dispatcher thread; hand the state to the main thread and return
    /// promptly.
    func authStateChanged(state: AuthState)

    /// Read the opaque core-owned SSO session blob from host-global storage.
    func readSession() throws -> Data?

    /// Persist the opaque core-owned SSO session blob in host-global storage.
    func writeSession(value: Data) throws

    /// Clear the persisted core-owned SSO session blob.
    func clearSession() throws

    /// Open a JSON-RPC chain connection and return a host-assigned id, or nil if unsupported.
    func chainConnect(genesisHash: Data) throws -> UInt32?

    /// Send one JSON-RPC request on a native chain connection.
    func chainSend(connectionId: UInt32, request: String) throws

    /// Close a native chain connection.
    func chainClose(connectionId: UInt32) throws

    /// Confirm a sign-payload request before the core asks the SSO peer.
    func confirmSignPayload(review: Data) throws -> Bool

    /// Confirm a sign-raw request before the core asks the SSO peer.
    func confirmSignRaw(review: Data) throws -> Bool

    /// Confirm a create-transaction request before the core asks the SSO peer.
    func confirmCreateTransaction(review: Data) throws -> Bool

    /// Confirm a cross-domain account-alias request before the core asks the SSO peer.
    func confirmAccountAlias(review: Data) throws -> Bool

    /// Confirm a resource-allocation request before the core asks the SSO peer.
    func confirmResourceAllocation(review: Data) throws -> Bool

    /// Confirm preimage submission before the host stores it.
    func confirmPreimageSubmit(size: UInt64) throws

    /// Submit a preimage through the host backend and return its key.
    func submitPreimage(value: Data) throws -> Data

    /// Return the current preimage value for `key`, or nil for a miss.
    func lookupPreimage(key: Data) throws -> Data?

    /// Return the current host theme.
    func currentTheme() throws -> HostTheme

    /// Answer whether a chain is supported. Invoked on the
    /// `truapi-ws-bridge` worker thread.
    func featureSupportedChain(genesisHash: Data) throws -> Bool

    /// Create or join a chat room and return the registration status.
    func chatCreateRoom(roomId: String, name: String, icon: String) throws -> HostChatRoomRegistrationStatus

    /// Post a text message into the host chat system and return the
    /// host-assigned message id.
    func chatPostTextMessage(roomId: String, text: String) throws -> String

    /// Post a custom message into the host chat system and return the
    /// host-assigned message id.
    func chatPostCustomMessage(roomId: String, messageType: String, payload: Data) throws -> String

    /// Start forwarding payment balance changes and return the first balance.
    func paymentBalanceSubscribe() throws -> String

    /// Initiate a payment and return the host-assigned payment id. Invoked on
    /// a blocking-pool thread because the host may present approval UI.
    func paymentRequest(amountInPlanks: String, destination: Data) throws -> String

    /// Top up the product payment balance. Invoked on a blocking-pool thread
    /// because the host may present approval UI.
    func paymentTopUp(amountInPlanks: String, source: NativePaymentTopUpSource) throws

    /// Start forwarding status changes for a payment and return its current status.
    func paymentStatusSubscribe(paymentId: String) throws -> NativePaymentStatus

    /// Scoped key-value storage for the Rust core.
    var storage: HostStorageBackend { get }
}

public extension HostBridge {
    /// Default no-op logger. Override to plumb into your logging framework.
    func onCoreLog(marker: String, detail: String) {}
    func pushNotification(text: String, deeplink: String?, scheduledAtMs: UInt64?) throws -> UInt32 { 0 }
    func cancelNotification(id: UInt32) throws {}
    func authStateChanged(state: AuthState) {}
    func readSession() throws -> Data? { nil }
    func writeSession(value: Data) throws {}
    func clearSession() throws {}
    func chainConnect(genesisHash: Data) throws -> UInt32? { nil }
    func chainSend(connectionId: UInt32, request: String) throws {}
    func chainClose(connectionId: UInt32) throws {}
    func confirmSignPayload(review: Data) throws -> Bool { false }
    func confirmSignRaw(review: Data) throws -> Bool { false }
    func confirmCreateTransaction(review: Data) throws -> Bool { false }
    func confirmAccountAlias(review: Data) throws -> Bool { false }
    func confirmResourceAllocation(review: Data) throws -> Bool { false }
    func confirmPreimageSubmit(size: UInt64) throws {}
    func submitPreimage(value: Data) throws -> Data { value }
    func lookupPreimage(key: Data) throws -> Data? { nil }
    func currentTheme() throws -> HostTheme { .dark }
    func chatPostTextMessage(roomId: String, text: String) throws -> String {
        throw HostRejection.Rejected(reason: "chat posting unavailable")
    }
    func chatPostCustomMessage(roomId: String, messageType: String, payload: Data) throws -> String {
        throw HostRejection.Rejected(reason: "chat posting unavailable")
    }
    func paymentBalanceSubscribe() throws -> String {
        throw HostPaymentBalanceSubscriptionRejection.Unknown(reason: "payment balance unavailable")
    }
    func paymentRequest(amountInPlanks: String, destination: Data) throws -> String {
        throw HostPaymentRequestRejection.Unknown(reason: "payment request unavailable")
    }
    func paymentTopUp(amountInPlanks: String, source: NativePaymentTopUpSource) throws {
        throw HostPaymentTopUpRejection.Unknown(reason: "payment top-up unavailable")
    }
    func paymentStatusSubscribe(paymentId: String) throws -> NativePaymentStatus {
        throw HostPaymentStatusSubscriptionRejection.Unknown(reason: "payment status unavailable")
    }
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

    func navigateTo(url: String) throws {
        try bridge.navigateTo(url: url)
    }

    func pushNotification(text: String, deeplink: String?, scheduledAtMs: UInt64?) throws -> UInt32 {
        try bridge.pushNotification(text: text, deeplink: deeplink, scheduledAtMs: scheduledAtMs)
    }

    func cancelNotification(id: UInt32) throws {
        try bridge.cancelNotification(id: id)
    }

    func devicePermission(capability: String) throws -> Bool {
        try bridge.devicePermission(capability: capability)
    }

    func remotePermission(permission: String, domains: [String]) throws -> Bool {
        try bridge.remotePermission(permission: permission, domains: domains)
    }

    func authStateChanged(state: AuthState) {
        bridge.authStateChanged(state: state)
    }

    func readSession() throws -> Data? {
        try bridge.readSession()
    }

    func writeSession(value: Data) throws {
        try bridge.writeSession(value: value)
    }

    func clearSession() throws {
        try bridge.clearSession()
    }

    func chainConnect(genesisHash: Data) throws -> UInt32? {
        try bridge.chainConnect(genesisHash: genesisHash)
    }

    func chainSend(connectionId: UInt32, request: String) throws {
        try bridge.chainSend(connectionId: connectionId, request: request)
    }

    func chainClose(connectionId: UInt32) throws {
        try bridge.chainClose(connectionId: connectionId)
    }

    func confirmSignPayload(review: Data) throws -> Bool {
        try bridge.confirmSignPayload(review: review)
    }

    func confirmSignRaw(review: Data) throws -> Bool {
        try bridge.confirmSignRaw(review: review)
    }

    func confirmCreateTransaction(review: Data) throws -> Bool {
        try bridge.confirmCreateTransaction(review: review)
    }

    func confirmAccountAlias(review: Data) throws -> Bool {
        try bridge.confirmAccountAlias(review: review)
    }

    func confirmResourceAllocation(review: Data) throws -> Bool {
        try bridge.confirmResourceAllocation(review: review)
    }

    func confirmPreimageSubmit(size: UInt64) throws {
        try bridge.confirmPreimageSubmit(size: size)
    }

    func submitPreimage(value: Data) throws -> Data {
        try bridge.submitPreimage(value: value)
    }

    func lookupPreimage(key: Data) throws -> Data? {
        try bridge.lookupPreimage(key: key)
    }

    func currentTheme() throws -> HostTheme {
        try bridge.currentTheme()
    }

    func featureSupportedChain(genesisHash: Data) throws -> Bool {
        try bridge.featureSupportedChain(genesisHash: genesisHash)
    }

    func chatCreateRoom(roomId: String, name: String, icon: String) throws -> HostChatRoomRegistrationStatus {
        try bridge.chatCreateRoom(roomId: roomId, name: name, icon: icon)
    }

    func chatPostTextMessage(roomId: String, text: String) throws -> String {
        try bridge.chatPostTextMessage(roomId: roomId, text: text)
    }

    func chatPostCustomMessage(roomId: String, messageType: String, payload: Data) throws -> String {
        try bridge.chatPostCustomMessage(roomId: roomId, messageType: messageType, payload: payload)
    }

    func paymentBalanceSubscribe() throws -> String {
        try bridge.paymentBalanceSubscribe()
    }

    func paymentRequest(amountInPlanks: String, destination: Data) throws -> String {
        try bridge.paymentRequest(amountInPlanks: amountInPlanks, destination: destination)
    }

    func paymentTopUp(amountInPlanks: String, source: NativePaymentTopUpSource) throws {
        try bridge.paymentTopUp(amountInPlanks: amountInPlanks, source: source)
    }

    func paymentStatusSubscribe(paymentId: String) throws -> NativePaymentStatus {
        try bridge.paymentStatusSubscribe(paymentId: paymentId)
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
/// that URL into `@parity/truapi`'s `createWebSocketProvider`.
public final class TrUAPIHostCore: @unchecked Sendable {
    private let inner: NativeTrUApiCore
    // Co-owns the adapter alongside the generated FfiConverter handle map,
    // which is what actually keeps the callback object alive for the core.
    private let callbackRetainer: HostCallbacks

    public init(bridge: HostBridge, runtimeConfig: RuntimeConfig) throws {
        let adapter = HostCallbackAdapter(bridge: bridge)
        self.callbackRetainer = adapter
        self.inner = try NativeTrUApiCore.withRuntimeConfig(
            callbacks: adapter,
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
    /// ``HostBridge/clearSession()``, and broadcasts `Disconnected` to active
    /// account-status subscribers.
    public func disconnect() {
        inner.disconnect()
    }

    /// Notify the core that host-global session storage changed externally.
    public func notifySessionStoreChanged() {
        inner.notifySessionStoreChanged()
    }

    /// Cancel any in-flight login pairing (e.g. the user dismissed the
    /// pairing UI). The bridge receives a `.disconnected` auth state
    /// immediately and the pending login resolves as rejected. A no-op when
    /// no login is in progress.
    public func cancelLogin() {
        inner.cancelLogin()
    }

    /// Push a host theme update to active TrUAPI theme subscriptions.
    public func notifyThemeChanged(theme: HostTheme) {
        inner.notifyThemeChanged(theme: theme)
    }

    /// Push a payment balance update to active `payment.balanceSubscribe()` subscriptions.
    public func notifyPaymentBalanceChanged(amountInPlanks: String) {
        inner.notifyPaymentBalanceChanged(amountInPlanks: amountInPlanks)
    }

    /// Push a payment status update to active `payment.statusSubscribe()` subscriptions.
    public func notifyPaymentStatusChanged(paymentId: String, status: NativePaymentStatus) {
        inner.notifyPaymentStatusChanged(paymentId: paymentId, status: status)
    }

    /// Push a host-originated plain-text chat message to active
    /// `chat.actionSubscribe()` subscriptions.
    public func notifyChatMessagePosted(roomId: String, peer: String, text: String) {
        inner.notifyChatMessagePosted(roomId: roomId, peer: peer, text: text)
    }

    /// Push a host-originated custom chat message to active
    /// `chat.actionSubscribe()` subscriptions.
    public func notifyChatCustomMessagePosted(
        roomId: String,
        peer: String,
        messageType: String,
        payload: Data
    ) {
        inner.notifyChatCustomMessagePosted(
            roomId: roomId,
            peer: peer,
            messageType: messageType,
            payload: payload
        )
    }

    /// Push a host-originated chat action trigger to active
    /// `chat.actionSubscribe()` subscriptions.
    public func notifyChatActionTriggered(
        roomId: String,
        peer: String,
        messageId: String,
        actionId: String,
        payload: Data?
    ) {
        inner.notifyChatActionTriggered(
            roomId: roomId,
            peer: peer,
            messageId: messageId,
            actionId: actionId,
            payload: payload
        )
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

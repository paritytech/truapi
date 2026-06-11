// TrUAPIHost - Android host adapter.
//
// The Rust core (compiled to `libtruapi_server.so` and surfaced via UniFFI in
// `src/main/kotlin/generated/uniffi/truapi_server/truapi_server.kt`) owns the
// wire protocol, request routing, subscription lifecycle, and platform trait
// dispatch.
//
// This file exposes:
//
//   * `HostBridge` - the Kotlin-friendly callback interface the embedding app
//     implements. It splits device and remote permissions, mirroring the
//     `Permissions` platform trait in the Rust core.
//   * `TrUAPIHostCore` - owning wrapper around the UniFFI-generated
//     `NativeTrUApiCore`. Holds the bridge alive for the lifetime of the
//     core and exposes session + WS-bridge controls.
//
// Products running inside a `WebView` connect to the Rust core via the
// localhost WebSocket bridge. Start it with `core.startWsBridge()` and load
// the product page with the resulting `ws://127.0.0.1:<port>/?t=<token>` URL
// passed through to the product's `@parity/truapi` `createWebSocketProvider`
// call.

package io.parity.truapi

import uniffi.truapi_server.AuthState
import uniffi.truapi_server.HostCallbacks
import uniffi.truapi_server.HostNavigateRejection
import uniffi.truapi_server.HostRejection
import uniffi.truapi_server.HostStorageException
import uniffi.truapi_server.HostTheme
import uniffi.truapi_server.NativeTrUApiCore
import uniffi.truapi_server.NativePairingDeeplinkScheme as UniFfiNativePairingDeeplinkScheme
import uniffi.truapi_server.NativeRuntimeConfig as UniFfiNativeRuntimeConfig
import uniffi.truapi_server.NativeRuntimeConfigException
import uniffi.truapi_server.WsBridgeEndpoint
import uniffi.truapi_server.WsBridgeStartException

/** Package metadata. */
object TrUAPIHost {
    const val VERSION = "0.1.0"
}

/** Deeplink scheme used when the Rust core builds SSO pairing payloads. */
enum class PairingDeeplinkScheme {
    POLKADOT_APP,
    POLKADOT_APP_DEV;

    internal fun toNative(): UniFfiNativePairingDeeplinkScheme =
        when (this) {
            POLKADOT_APP -> UniFfiNativePairingDeeplinkScheme.POLKADOT_APP
            POLKADOT_APP_DEV -> UniFfiNativePairingDeeplinkScheme.POLKADOT_APP_DEV
        }
}

/**
 * Static product and pairing config supplied before the Rust core handles
 * product calls. One core instance represents one product identity.
 *
 * [hostName], [hostIcon], [hostVersion], [platformType], and
 * [platformVersion] describe the host to the wallet during SSO pairing.
 * [peopleChainGenesisHash] must be exactly 32 bytes.
 */
data class RuntimeConfig(
    val productLabel: String,
    val productId: String,
    val siteId: String,
    val hostName: String,
    val hostIcon: String? = null,
    val hostVersion: String? = null,
    val platformType: String? = null,
    val platformVersion: String? = null,
    val peopleChainGenesisHash: ByteArray,
    val pairingDeeplinkScheme: PairingDeeplinkScheme = PairingDeeplinkScheme.POLKADOT_APP,
) {
    internal fun toNative(): UniFfiNativeRuntimeConfig =
        UniFfiNativeRuntimeConfig(
            productLabel = productLabel,
            productId = productId,
            siteId = siteId,
            hostName = hostName,
            hostIcon = hostIcon,
            hostVersion = hostVersion,
            platformType = platformType,
            platformVersion = platformVersion,
            peopleChainGenesisHash = peopleChainGenesisHash,
            pairingDeeplinkScheme = pairingDeeplinkScheme.toNative(),
        )

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is RuntimeConfig) return false
        return productLabel == other.productLabel &&
            productId == other.productId &&
            siteId == other.siteId &&
            hostName == other.hostName &&
            hostIcon == other.hostIcon &&
            hostVersion == other.hostVersion &&
            platformType == other.platformType &&
            platformVersion == other.platformVersion &&
            peopleChainGenesisHash.contentEquals(other.peopleChainGenesisHash) &&
            pairingDeeplinkScheme == other.pairingDeeplinkScheme
    }

    override fun hashCode(): Int {
        var result = productLabel.hashCode()
        result = 31 * result + productId.hashCode()
        result = 31 * result + siteId.hashCode()
        result = 31 * result + hostName.hashCode()
        result = 31 * result + (hostIcon?.hashCode() ?: 0)
        result = 31 * result + (hostVersion?.hashCode() ?: 0)
        result = 31 * result + (platformType?.hashCode() ?: 0)
        result = 31 * result + (platformVersion?.hashCode() ?: 0)
        result = 31 * result + peopleChainGenesisHash.contentHashCode()
        result = 31 * result + pairingDeeplinkScheme.hashCode()
        return result
    }
}

/**
 * Storage backend the host provides to the Rust core. Throws
 * [HostStorageException] to signal quota exhaustion or unknown failure; the
 * core maps both onto the v0.1 `HostLocalStorageReadError` wire shape.
 */
interface HostStorage {
    @Throws(HostStorageException::class)
    fun read(key: String): ByteArray?

    @Throws(HostStorageException::class)
    fun write(key: String, value: ByteArray)

    @Throws(HostStorageException::class)
    fun clear(key: String)
}

/**
 * Host-side callback bundle that the Rust core invokes for capabilities the
 * native shell owns. The interface mirrors the underlying UniFFI surface
 * but keeps the permission split explicit:
 *
 *   * [devicePermission] handles camera / mic / push prompts and similar
 *     OS-scoped grants. `request` is a SCALE-encoded
 *     `v01::HostDevicePermissionRequest`.
 *   * [remotePermission] handles per-product capability bundles requested
 *     by the application running inside the WebView. `request` is a
 *     SCALE-encoded `v01::RemotePermissionRequest`.
 *
 * Embedders typically wire the SCALE payloads through the generated
 * `@parity/truapi` client running on the JS side for UI rendering, then
 * report the user's decision as a `Boolean`.
 *
 * Threading: the Rust core invokes every callback on a background thread it
 * owns, never the UI (main) thread. UI-decision callbacks ([navigateTo],
 * [devicePermission], [remotePermission], the `confirm*` family, and
 * [submitPreimage]) each run on their own thread from a blocking pool, so an
 * implementation may safely block its calling thread (e.g. with a
 * `CountDownLatch`) until the user decides; other TrUAPI traffic keeps
 * flowing. The remaining callbacks (auth state, storage, session, chain,
 * feature, theme, preimage lookups) run inline on the dispatcher thread and
 * must return promptly without blocking. Any UI work MUST still be marshalled
 * onto the main thread, e.g. with `Handler(Looper.getMainLooper()).post { ... }`
 * or a `CoroutineScope` bound to `Dispatchers.Main`. Touching views or the
 * `WebView` directly from a callback throws `CalledFromWrongThreadException`.
 */
interface HostBridge {
    /** Lifecycle logger. Marker is a stable slug, detail is free-form. */
    fun onCoreLog(marker: String, detail: String) {}

    /**
     * Open a URL in the system browser. Invoked on a blocking-pool thread;
     * marshal the UI launch (e.g. `startActivity`) to the main thread. May
     * block the calling thread if the user has to approve the navigation.
     */
    @Throws(HostNavigateRejection::class)
    fun navigateTo(url: String)

    /**
     * Deliver a push notification (SCALE-encoded `HostPushNotificationRequest`)
     * and return the host-assigned notification id. Invoked on the dispatcher
     * thread; marshal any UI work to the main thread and return promptly.
     */
    @Throws(HostRejection::class)
    fun pushNotification(payload: ByteArray): UInt = 0u

    /** Cancel a previously scheduled notification id. */
    @Throws(HostRejection::class)
    fun cancelNotification(id: UInt) {}

    /**
     * Prompt for a device-level permission. Returns whether it was granted.
     * Invoked on a blocking-pool thread; present the prompt on the main
     * thread and block the calling thread until the user decides. Blocking
     * here does not stall other TrUAPI traffic.
     */
    @Throws(HostRejection::class)
    fun devicePermission(request: ByteArray): Boolean

    /**
     * Prompt for a remote (product-scoped) permission bundle. Invoked on a
     * blocking-pool thread; present the prompt on the main thread and block
     * the calling thread until the user decides. Blocking here does not stall
     * other TrUAPI traffic.
     */
    @Throws(HostRejection::class)
    fun remotePermission(request: ByteArray): Boolean

    /**
     * Observe an auth state change. The core emits states only when they
     * actually change, in transition order: render [AuthState.Pairing]
     * as the pairing QR UI, connected/disconnected as the account badge, and
     * login-failed as a retryable error. Report a user dismissal of the
     * pairing UI through [TrUAPIHostCore.cancelLogin]. Invoked on the
     * dispatcher thread; marshal the state to the main thread and return
     * promptly.
     */
    fun authStateChanged(state: AuthState) {}

    /** Read the opaque core-owned SSO session blob from host-global storage. */
    @Throws(HostRejection::class)
    fun readSession(): ByteArray? = null

    /** Persist the opaque core-owned SSO session blob in host-global storage. */
    @Throws(HostRejection::class)
    fun writeSession(value: ByteArray) {}

    /** Clear the persisted core-owned SSO session blob. */
    @Throws(HostRejection::class)
    fun clearSession() {}

    /** Open a JSON-RPC chain connection and return a host-assigned id, or null if unsupported. */
    @Throws(HostRejection::class)
    fun chainConnect(genesisHash: ByteArray): UInt? = null

    /** Send one JSON-RPC request on a native chain connection. */
    @Throws(HostRejection::class)
    fun chainSend(connectionId: UInt, request: String) {}

    /** Close a native chain connection. */
    @Throws(HostRejection::class)
    fun chainClose(connectionId: UInt) {}

    /** Confirm a sign-payload request before the core asks the SSO peer. */
    @Throws(HostRejection::class)
    fun confirmSignPayload(review: ByteArray): Boolean = false

    /** Confirm a sign-raw request before the core asks the SSO peer. */
    @Throws(HostRejection::class)
    fun confirmSignRaw(review: ByteArray): Boolean = false

    /** Confirm a create-transaction request before the core asks the SSO peer. */
    @Throws(HostRejection::class)
    fun confirmCreateTransaction(review: ByteArray): Boolean = false

    /** Confirm a cross-domain account-alias request before the core asks the SSO peer. */
    @Throws(HostRejection::class)
    fun confirmAccountAlias(review: ByteArray): Boolean = false

    /** Confirm a resource-allocation request before the core asks the SSO peer. */
    @Throws(HostRejection::class)
    fun confirmResourceAllocation(review: ByteArray): Boolean = false

    /** Confirm preimage submission before the host stores it. */
    @Throws(HostRejection::class)
    fun confirmPreimageSubmit(size: ULong) {}

    /** Submit a preimage through the host backend and return its key. */
    @Throws(HostRejection::class)
    fun submitPreimage(value: ByteArray): ByteArray = value

    /** Return the current preimage value for [key], or null for a miss. */
    @Throws(HostRejection::class)
    fun lookupPreimage(key: ByteArray): ByteArray? = null

    /** Return the current host theme. */
    @Throws(HostRejection::class)
    fun currentTheme(): HostTheme = HostTheme.DARK

    /**
     * Answer a feature-support query. Invoked on the dispatcher thread; must
     * return promptly.
     */
    @Throws(HostRejection::class)
    fun featureSupported(request: ByteArray): Boolean

    /** Scoped key-value storage for the Rust core. */
    val storage: HostStorage
}

/**
 * Adapter from the public [HostBridge] surface to the generated UniFFI
 * [HostCallbacks] interface. Keeps the public API stable even if uniffi-bindgen
 * renames generated symbols.
 */
private class HostCallbackAdapter(private val bridge: HostBridge) : HostCallbacks {
    override fun onCoreLog(marker: String, detail: String) =
        bridge.onCoreLog(marker, detail)

    override fun navigateTo(url: String) =
        bridge.navigateTo(url)

    override fun pushNotification(payload: ByteArray): UInt =
        bridge.pushNotification(payload)

    override fun cancelNotification(id: UInt) =
        bridge.cancelNotification(id)

    override fun devicePermission(request: ByteArray): Boolean =
        bridge.devicePermission(request)

    override fun remotePermission(request: ByteArray): Boolean =
        bridge.remotePermission(request)

    override fun authStateChanged(state: AuthState) =
        bridge.authStateChanged(state)

    override fun readSession(): ByteArray? =
        bridge.readSession()

    override fun writeSession(value: ByteArray) =
        bridge.writeSession(value)

    override fun clearSession() =
        bridge.clearSession()

    override fun chainConnect(genesisHash: ByteArray): UInt? =
        bridge.chainConnect(genesisHash)

    override fun chainSend(connectionId: UInt, request: String) =
        bridge.chainSend(connectionId, request)

    override fun chainClose(connectionId: UInt) =
        bridge.chainClose(connectionId)

    override fun confirmSignPayload(review: ByteArray): Boolean =
        bridge.confirmSignPayload(review)

    override fun confirmSignRaw(review: ByteArray): Boolean =
        bridge.confirmSignRaw(review)

    override fun confirmCreateTransaction(review: ByteArray): Boolean =
        bridge.confirmCreateTransaction(review)

    override fun confirmAccountAlias(review: ByteArray): Boolean =
        bridge.confirmAccountAlias(review)

    override fun confirmResourceAllocation(review: ByteArray): Boolean =
        bridge.confirmResourceAllocation(review)

    override fun confirmPreimageSubmit(size: ULong) =
        bridge.confirmPreimageSubmit(size)

    override fun submitPreimage(value: ByteArray): ByteArray =
        bridge.submitPreimage(value)

    override fun lookupPreimage(key: ByteArray): ByteArray? =
        bridge.lookupPreimage(key)

    override fun currentTheme(): HostTheme =
        bridge.currentTheme()

    override fun featureSupported(request: ByteArray): Boolean =
        bridge.featureSupported(request)

    override fun localStorageRead(key: String): ByteArray? =
        bridge.storage.read(key)

    override fun localStorageWrite(key: String, value: ByteArray) =
        bridge.storage.write(key, value)

    override fun localStorageClear(key: String) =
        bridge.storage.clear(key)
}

/**
 * Owning wrapper around the Rust-backed [NativeTrUApiCore]. Holds the bridge
 * alive for the lifetime of the core and exposes core lifecycle + WS-bridge
 * controls.
 *
 * Hosts integrating with a `WebView`-based product call [startWsBridge] and
 * pass the resulting `ws://127.0.0.1:<port>/?t=<token>` URL to the product
 * (typically via a query string or page-bootstrap hook). The product wires
 * that URL into `@parity/truapi`'s `createWebSocketProvider`.
 */
class TrUAPIHostCore private constructor(
    bridge: HostBridge,
    runtimeConfig: UniFfiNativeRuntimeConfig,
) : AutoCloseable {
    @Throws(NativeRuntimeConfigException::class)
    constructor(bridge: HostBridge, runtimeConfig: RuntimeConfig) : this(
        bridge,
        runtimeConfig.toNative(),
    )

    // Co-owns the adapter alongside the generated FfiConverter handle map,
    // which is what actually keeps the callback object alive for the core.
    private val callbackRetainer: HostCallbacks = HostCallbackAdapter(bridge)
    private val inner: NativeTrUApiCore =
        NativeTrUApiCore.withRuntimeConfig(callbackRetainer, runtimeConfig)

    /**
     * Start the localhost WebSocket bridge (requires the `ws-bridge` feature
     * in the cdylib). The returned [WsBridgeEndpoint] carries the port and
     * session token; build a `ws://127.0.0.1:<port>/?t=<token>` URL and pass
     * it to the product's `createWebSocketProvider` call.
     */
    @Throws(WsBridgeStartException::class)
    fun startWsBridge(bindPort: UShort = 0u): WsBridgeEndpoint =
        inner.startWsBridge(bindPort)

    /** Stop the localhost WebSocket bridge (if running). */
    fun stopWsBridge() {
        inner.stopWsBridge()
    }

    /**
     * Core-owned logout/disconnect path. Best-effort notifies the SSO peer,
     * clears in-memory session state, clears the persisted session via
     * [HostBridge.clearSession], and broadcasts `Disconnected` to active
     * account-status subscribers.
     */
    fun disconnect() {
        inner.disconnect()
    }

    /** Notify the core that host-global session storage changed externally. */
    fun notifySessionStoreChanged() {
        inner.notifySessionStoreChanged()
    }

    /**
     * Cancel any in-flight login pairing (e.g. the user dismissed the pairing
     * UI). The bridge receives a disconnected auth state immediately and the
     * pending login resolves as rejected. A no-op when no login is in
     * progress.
     */
    fun cancelLogin() {
        inner.cancelLogin()
    }

    /** Push a host theme update to active TrUAPI theme subscriptions. */
    fun notifyThemeChanged(theme: HostTheme) {
        inner.notifyThemeChanged(theme)
    }

    /** Push a preimage lookup update to active subscriptions for [key]. */
    fun notifyPreimageChanged(key: ByteArray, value: ByteArray?) {
        inner.notifyPreimageChanged(key, value)
    }

    /** Push a JSON-RPC response from a native chain connection into the core. */
    fun notifyChainResponse(connectionId: UInt, json: String) {
        inner.notifyChainResponse(connectionId, json)
    }

    /** Notify the core that a native chain connection closed externally. */
    fun notifyChainClosed(connectionId: UInt) {
        inner.notifyChainClosed(connectionId)
    }

    override fun close() {
        inner.close()
    }
}

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

import uniffi.truapi_server.HostCallbacks
import uniffi.truapi_server.HostNavigateRejection
import uniffi.truapi_server.HostRejection
import uniffi.truapi_server.HostStorageException
import uniffi.truapi_server.NativeTrUApiCore
import uniffi.truapi_server.WsBridgeEndpoint
import uniffi.truapi_server.WsBridgeStartException

/** Package metadata. */
object TrUAPIHost {
    const val VERSION = "0.1.0"
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
 * Threading: when the WS bridge is running, the Rust core invokes every
 * callback on the dedicated `truapi-ws-bridge` worker thread, never the UI
 * (main) thread. Any UI work an implementation does (navigation, prompts,
 * notifications) MUST be marshalled onto the main thread, e.g. with
 * `Handler(Looper.getMainLooper()).post { ... }` or a `CoroutineScope` bound
 * to `Dispatchers.Main`. Touching views or the `WebView` directly from a
 * callback throws `CalledFromWrongThreadException`.
 */
interface HostBridge {
    /** Lifecycle logger. Marker is a stable slug, detail is free-form. */
    fun onCoreLog(marker: String, detail: String) {}

    /**
     * Open a URL in the system browser. Invoked on the `truapi-ws-bridge`
     * worker thread; marshal the UI launch (e.g. `startActivity`) to the main
     * thread.
     */
    @Throws(HostNavigateRejection::class)
    fun navigateTo(url: String)

    /**
     * Deliver a push notification (SCALE-encoded `HostPushNotificationRequest`).
     * Invoked on the `truapi-ws-bridge` worker thread; marshal any UI work to
     * the main thread.
     */
    @Throws(HostRejection::class)
    fun pushNotification(payload: ByteArray)

    /**
     * Prompt for a device-level permission. Returns whether it was granted.
     * Invoked on the `truapi-ws-bridge` worker thread; present the prompt on
     * the main thread and block this thread until the user decides.
     */
    @Throws(HostRejection::class)
    fun devicePermission(request: ByteArray): Boolean

    /**
     * Prompt for a remote (product-scoped) permission bundle. Invoked on the
     * `truapi-ws-bridge` worker thread; present the prompt on the main thread
     * and block this thread until the user decides.
     */
    @Throws(HostRejection::class)
    fun remotePermission(request: ByteArray): Boolean

    /**
     * Answer a feature-support query. Invoked on the `truapi-ws-bridge` worker
     * thread.
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

    override fun pushNotification(payload: ByteArray) =
        bridge.pushNotification(payload)

    override fun devicePermission(request: ByteArray): Boolean =
        bridge.devicePermission(request)

    override fun remotePermission(request: ByteArray): Boolean =
        bridge.remotePermission(request)

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
class TrUAPIHostCore(bridge: HostBridge) : AutoCloseable {
    // Co-owns the adapter alongside the generated FfiConverter handle map,
    // which is what actually keeps the callback object alive for the core.
    private val callbackRetainer: HostCallbacks = HostCallbackAdapter(bridge)
    private val inner: NativeTrUApiCore = NativeTrUApiCore(callbackRetainer)

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

    override fun close() {
        inner.close()
    }
}

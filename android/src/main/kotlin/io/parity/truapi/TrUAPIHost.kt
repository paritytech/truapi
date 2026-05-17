// TrUAPIHost - Android host adapter.
//
// The Rust core (compiled to `libtruapi_server.so` and surfaced via UniFFI in
// `src/main/kotlin/generated/uniffi/truapi_server/truapi_server.kt`) owns the
// wire protocol, request routing, subscription lifecycle, and platform trait
// dispatch.
//
// This file exposes two things on top of the generated bindings:
//
//   * `HostBridge` - a Kotlin-friendly callback interface the embedding app
//     implements. It splits device and remote permissions, mirroring the
//     `Permissions` platform trait in the Rust core.
//   * `WebViewTransport` - a thin byte transport that forwards opaque wire
//     bytes between a `WebView` and the Rust core. Bytes traverse the
//     `JavascriptInterface` boundary as base64 because the bridge cannot
//     carry binary types directly.
//
// The transport is independent of the core: tests can stand up a
// `WebViewTransport` against a non-UniFFI stub by implementing `CoreInbound`.

package io.parity.truapi

import android.util.Base64
import android.webkit.JavascriptInterface
import android.webkit.WebView
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
 */
interface HostBridge {
    /** Lifecycle logger. Marker is a stable slug, detail is free-form. */
    fun onCoreLog(marker: String, detail: String) {}

    /** Forward an outbound SCALE-encoded protocol frame to the product. */
    fun onCoreResponse(frame: ByteArray)

    /** Open a URL in the system browser. */
    @Throws(HostNavigateRejection::class)
    fun navigateTo(url: String)

    /** Deliver a push notification (SCALE-encoded `HostPushNotificationRequest`). */
    @Throws(HostRejection::class)
    fun pushNotification(payload: ByteArray)

    /** Prompt for a device-level permission. Returns whether it was granted. */
    @Throws(HostRejection::class)
    fun devicePermission(request: ByteArray): Boolean

    /** Prompt for a remote (product-scoped) permission bundle. */
    @Throws(HostRejection::class)
    fun remotePermission(request: ByteArray): Boolean

    /** Answer a feature-support query. */
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

    override fun onCoreResponse(frame: ByteArray) =
        bridge.onCoreResponse(frame)

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
 * Sink for opaque wire frames coming from the WebView. The Rust core is the
 * typical implementor (via [TrUAPIHostCore]); tests may use a stub.
 */
fun interface CoreInbound {
    fun receiveFromProduct(frame: ByteArray)
}

/**
 * Owning wrapper around the Rust-backed [NativeTrUApiCore]. Implements
 * [CoreInbound] so a [WebViewTransport] can deliver inbound frames directly,
 * and exposes session and WS bridge controls.
 *
 * The wrapper holds a strong reference to the bridge so the JNA callback
 * registration stays alive for the lifetime of the core.
 */
class TrUAPIHostCore(bridge: HostBridge) : CoreInbound, AutoCloseable {
    @Suppress("unused") // retained to keep JNA callbacks alive
    private val callbackRetainer: HostCallbacks = HostCallbackAdapter(bridge)
    private val inner: NativeTrUApiCore = NativeTrUApiCore(callbackRetainer)

    override fun receiveFromProduct(frame: ByteArray): Unit {
        inner.receiveFromProduct(frame)
    }

    /** Set the currently-paired session. `pubkey` must be exactly 32 bytes. */
    fun setActiveSession(
        pubkey: ByteArray,
        liteUsername: String? = null,
        fullUsername: String? = null,
    ): Boolean = inner.setActiveSession(pubkey, liteUsername, fullUsername)

    /** Drop the currently-paired session. */
    fun clearActiveSession() {
        inner.clearActiveSession()
    }

    /** Start the localhost WebSocket bridge (requires the `ws-bridge` feature in the cdylib). */
    @Throws(WsBridgeStartException::class)
    fun startWsBridge(bindPort: UShort = 0u): WsBridgeEndpoint =
        inner.startWsBridge(bindPort)

    /** Stop the localhost WebSocket bridge (if running). */
    fun stopWsBridge() {
        inner.stopWsBridge()
    }

    /** Smoke-test helper: returns a SCALE-encoded `feature_supported` request frame. */
    fun debugSmokeFeatureRequestFrame(): ByteArray =
        inner.debugSmokeFeatureRequestFrame()

    override fun close() {
        inner.close()
    }
}

/**
 * Wraps a [WebView] and forwards opaque wire bytes between JS and [core].
 * Attach with [attach] before loading the page so the JS shim is installed.
 */
class WebViewTransport(
    private val webView: WebView,
    private val core: CoreInbound,
    private val callbackName: String = "__trUApiReceive",
    private val interfaceName: String = "TrUApi",
) {
    fun attach() {
        webView.addJavascriptInterface(JsInterface(), interfaceName)
    }

    fun detach() {
        webView.removeJavascriptInterface(interfaceName)
    }

    /**
     * JS bootstrap to inject at document start so the page exposes a
     * `window.trUApi` byte-pipe matching the JS host adapter shape.
     */
    val bootstrapScript: String = """
        (function() {
          var listeners = [];
          window.$callbackName = function(b64) {
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
              window.$interfaceName.postMessage(toB64(bytes));
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
    """.trimIndent()

    /** Called when the core has bytes to push to the product app. */
    fun sendToProduct(frame: ByteArray) {
        val b64 = Base64.encodeToString(frame, Base64.NO_WRAP)
        val js = "window.$callbackName && window.$callbackName('$b64')"
        webView.post { webView.evaluateJavascript(js, null) }
    }

    private inner class JsInterface {
        @JavascriptInterface
        fun postMessage(b64: String) {
            val frame = try {
                Base64.decode(b64, Base64.NO_WRAP)
            } catch (_: IllegalArgumentException) {
                return
            }
            core.receiveFromProduct(frame)
        }
    }
}

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
//   * `HostStorage` / `HostCoreStorage` - the product-scoped and core-owned
//     key-value backends the host persists.
//   * `TrUAPIHostCore` - owning wrapper around the UniFFI-generated
//     `NativeTrUApiCore`. Holds the bridge alive for the lifetime of the core
//     and exposes session + WS-bridge controls plus native change notifications.
//   * `LocalhostBridgeBootstrap` - JS snippet that publishes the WS bridge
//     endpoint to the product page so it can dial back in.
//
// Products running inside a `WebView` connect to the Rust core via the
// localhost WebSocket bridge. Start it with `core.startWsBridge()` and load
// the product page with a `LocalhostBridgeBootstrap.script(...)` snippet
// injected at document start so the page's `@parity/truapi`
// `createWebSocketProvider` can dial `ws://127.0.0.1:<port>/?t=<token>`.

package io.parity.truapi

import uniffi.truapi_server.AuthState
import uniffi.truapi_server.HostCallbacks
import uniffi.truapi_server.HostNavigateRejection
import uniffi.truapi_server.HostRejection
import uniffi.truapi_server.HostStorageException
import uniffi.truapi_server.HostTheme
import uniffi.truapi_server.NativeTrUApiCore
import uniffi.truapi_server.NativePairingDeeplinkScheme as UniFfiNativePairingDeeplinkScheme
import uniffi.truapi_server.NativePermissionAuthorizationStatus
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
 * [hostName], [hostIcon], [hostVersion], [platformType], and [platformVersion]
 * describe the host to the wallet during SSO pairing.
 * [peopleChainGenesisHash] and [bulletinChainGenesisHash] must each be exactly
 * 32 bytes. [localSessionSecret] optionally activates a local signing session
 * from host-held BIP-39 entropy (no SSO pairing needed).
 */
data class RuntimeConfig(
    val productId: String,
    val hostName: String,
    val hostIcon: String? = null,
    val hostVersion: String? = null,
    val platformType: String? = null,
    val platformVersion: String? = null,
    val peopleChainGenesisHash: ByteArray,
    val bulletinChainGenesisHash: ByteArray,
    val localSessionSecret: ByteArray? = null,
    val localSessionLiteUsername: String? = null,
    val pairingDeeplinkScheme: PairingDeeplinkScheme = PairingDeeplinkScheme.POLKADOT_APP,
) {
    internal fun toNative(): UniFfiNativeRuntimeConfig =
        UniFfiNativeRuntimeConfig(
            productId = productId,
            hostName = hostName,
            hostIcon = hostIcon,
            hostVersion = hostVersion,
            platformType = platformType,
            platformVersion = platformVersion,
            peopleChainGenesisHash = peopleChainGenesisHash,
            bulletinChainGenesisHash = bulletinChainGenesisHash,
            localSessionSecret = localSessionSecret,
            localSessionLiteUsername = localSessionLiteUsername,
            pairingDeeplinkScheme = pairingDeeplinkScheme.toNative(),
        )

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is RuntimeConfig) return false
        return productId == other.productId &&
            hostName == other.hostName &&
            hostIcon == other.hostIcon &&
            hostVersion == other.hostVersion &&
            platformType == other.platformType &&
            platformVersion == other.platformVersion &&
            peopleChainGenesisHash.contentEquals(other.peopleChainGenesisHash) &&
            bulletinChainGenesisHash.contentEquals(other.bulletinChainGenesisHash) &&
            (localSessionSecret ?: ByteArray(0)).contentEquals(
                other.localSessionSecret ?: ByteArray(0),
            ) &&
            localSessionLiteUsername == other.localSessionLiteUsername &&
            pairingDeeplinkScheme == other.pairingDeeplinkScheme
    }

    override fun hashCode(): Int {
        var result = productId.hashCode()
        result = 31 * result + hostName.hashCode()
        result = 31 * result + (hostIcon?.hashCode() ?: 0)
        result = 31 * result + (hostVersion?.hashCode() ?: 0)
        result = 31 * result + (platformType?.hashCode() ?: 0)
        result = 31 * result + (platformVersion?.hashCode() ?: 0)
        result = 31 * result + peopleChainGenesisHash.contentHashCode()
        result = 31 * result + bulletinChainGenesisHash.contentHashCode()
        result = 31 * result + (localSessionSecret?.contentHashCode() ?: 0)
        result = 31 * result + (localSessionLiteUsername?.hashCode() ?: 0)
        result = 31 * result + pairingDeeplinkScheme.hashCode()
        return result
    }
}

/**
 * Product-scoped key-value storage the host provides to the Rust core. Throws
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
 * Core-owned key-value storage the host backs with its own persistence. The
 * core writes auth session, pairing identity, and persisted permission
 * decisions here; [key] is a SCALE-encoded `CoreStorageKey`. Throws
 * [HostRejection] on failure.
 */
interface HostCoreStorage {
    @Throws(HostRejection::class)
    fun read(key: ByteArray): ByteArray?

    @Throws(HostRejection::class)
    fun write(key: ByteArray, value: ByteArray)

    @Throws(HostRejection::class)
    fun clear(key: ByteArray)
}

/**
 * Host-side callback bundle that the Rust core invokes for capabilities the
 * native shell owns. The interface mirrors the underlying UniFFI surface but
 * keeps the permission split explicit:
 *
 *   * [devicePermission] handles camera / mic / push prompts and similar
 *     OS-scoped grants. `request` is a SCALE-encoded
 *     `v01::HostDevicePermissionRequest`.
 *   * [remotePermission] handles per-product capability bundles requested by
 *     the application running inside the WebView. `request` is a SCALE-encoded
 *     `v01::RemotePermissionRequest`.
 *
 * Embedders typically wire the SCALE payloads through the generated
 * `@parity/truapi` client running on the JS side for UI rendering, then report
 * the user's decision as a `Boolean`.
 *
 * Threading: the Rust core invokes every callback on a background thread it
 * owns, never the UI (main) thread. UI-decision callbacks ([navigateTo],
 * [devicePermission], [remotePermission], and [confirmUserAction]) each run on
 * their own thread from a blocking pool, so an implementation may safely block
 * its calling thread (e.g. with a `CountDownLatch`) until the user decides;
 * other TrUAPI traffic keeps flowing. The remaining callbacks (auth state,
 * storage, core storage, chain, feature, theme, preimage lookups) run inline on
 * the dispatcher thread and must return promptly without blocking. Any UI work
 * MUST still be marshalled onto the main thread, e.g. with
 * `Handler(Looper.getMainLooper()).post { ... }` or a `CoroutineScope` bound to
 * `Dispatchers.Main`. Touching views or the `WebView` directly from a callback
 * throws `CalledFromWrongThreadException`.
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
     * Invoked on a blocking-pool thread; present the prompt on the main thread
     * and block the calling thread until the user decides. Blocking here does
     * not stall other TrUAPI traffic.
     */
    @Throws(HostRejection::class)
    fun devicePermission(request: ByteArray): Boolean

    /**
     * Prompt for a remote (product-scoped) permission bundle. Invoked on a
     * blocking-pool thread; present the prompt on the main thread and block the
     * calling thread until the user decides. Blocking here does not stall other
     * TrUAPI traffic.
     */
    @Throws(HostRejection::class)
    fun remotePermission(request: ByteArray): Boolean

    /**
     * Observe an auth state change. The core emits states only when they
     * actually change, in transition order: render [AuthState.Pairing] as the
     * pairing QR UI, connected/disconnected as the account badge, and
     * login-failed as a retryable error. Report a user dismissal of the pairing
     * UI through [TrUAPIHostCore.cancelLogin]. Invoked on the dispatcher thread;
     * marshal the state to the main thread and return promptly.
     */
    fun authStateChanged(state: AuthState) {}

    /** Open a JSON-RPC chain connection and return a host-assigned id, or null if unsupported. */
    @Throws(HostRejection::class)
    fun chainConnect(genesisHash: ByteArray): UInt? = null

    /** Send one JSON-RPC request on a native chain connection. */
    @Throws(HostRejection::class)
    fun chainSend(connectionId: UInt, request: String) {}

    /** Close a native chain connection. */
    @Throws(HostRejection::class)
    fun chainClose(connectionId: UInt) {}

    /**
     * Confirm one user-reviewed core action. `review` is a SCALE-encoded
     * `UserConfirmationReview`; decode it to pick the prompt (sign payload,
     * sign raw, create transaction, account alias, resource allocation, or
     * preimage submit). Invoked on a blocking-pool thread; present the prompt on
     * the main thread and block the calling thread until the user decides.
     */
    @Throws(HostRejection::class)
    fun confirmUserAction(review: ByteArray): Boolean = false

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

    /** Product-scoped key-value storage for the Rust core. */
    val storage: HostStorage

    /** Core-owned key-value storage for auth session / pairing identity / permission decisions. */
    val coreStorage: HostCoreStorage
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

    override fun coreStorageRead(key: ByteArray): ByteArray? =
        bridge.coreStorage.read(key)

    override fun coreStorageWrite(key: ByteArray, value: ByteArray) =
        bridge.coreStorage.write(key, value)

    override fun coreStorageClear(key: ByteArray) =
        bridge.coreStorage.clear(key)

    override fun chainConnect(genesisHash: ByteArray): UInt? =
        bridge.chainConnect(genesisHash)

    override fun chainSend(connectionId: UInt, request: String) =
        bridge.chainSend(connectionId, request)

    override fun chainClose(connectionId: UInt) =
        bridge.chainClose(connectionId)

    override fun confirmUserAction(review: ByteArray): Boolean =
        bridge.confirmUserAction(review)

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
 * Bootstrap helper for the native localhost WebSocket bridge that the Rust core
 * stands up via [TrUAPIHostCore.startWsBridge] when the cdylib is built with the
 * `ws-bridge` feature.
 */
object LocalhostBridgeBootstrap {
    /**
     * Returns a `<script>`-injectable snippet that publishes the endpoint
     * metadata on `window.__truapi_localhost`, exposes the legacy
     * `window.__HOST_API_PORT__` webview transport shape, and fires a
     * `truapi-native-ready` event. Inject at document start (before the product
     * page scripts run) so the page can dial the bridge immediately.
     */
    fun script(port: UShort, token: String): String {
        val url = "ws://127.0.0.1:$port/?t=$token"
        val safeUrl = jsStringLiteral(url)
        val safeToken = jsStringLiteral(token)
        return """
        (function() {
          var endpoint = { url: $safeUrl, token: $safeToken };

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
        """.trimIndent()
    }

    /**
     * Encodes [value] as a complete double-quoted JavaScript string literal,
     * safe to embed inside a `<script>` body. Escapes quotes, backslashes,
     * control characters, `/` (closing `</script` tags), and the U+2028 /
     * U+2029 line terminators that JS treats as newlines.
     */
    private fun jsStringLiteral(value: String): String {
        val sb = StringBuilder(value.length + 2)
        sb.append('"')
        for (ch in value) {
            when (ch.code) {
                '"'.code -> sb.append("\\\"")
                '\\'.code -> sb.append("\\\\")
                '/'.code -> sb.append("\\/")
                0x0A -> sb.append("\\n")
                0x0D -> sb.append("\\r")
                0x09 -> sb.append("\\t")
                0x08 -> sb.append("\\b")
                0x0C -> sb.append("\\f")
                0x2028 -> sb.append("\\u2028")
                0x2029 -> sb.append("\\u2029")
                else ->
                    if (ch.code < 0x20) {
                        sb.append("\\u")
                        sb.append(ch.code.toString(16).padStart(4, '0'))
                    } else {
                        sb.append(ch)
                    }
            }
        }
        sb.append('"')
        return sb.toString()
    }
}

/**
 * Owning wrapper around the Rust-backed [NativeTrUApiCore]. Holds the bridge
 * alive for the lifetime of the core and exposes core lifecycle + WS-bridge
 * controls plus native change notifications.
 *
 * Hosts integrating with a `WebView`-based product call [startWsBridge] and
 * inject a [LocalhostBridgeBootstrap.script] snippet at document start so the
 * product's `@parity/truapi` `createWebSocketProvider` dials
 * `ws://127.0.0.1:<port>/?t=<token>`.
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
     * Start the localhost WebSocket bridge (requires the `ws-bridge` feature in
     * the cdylib). The returned [WsBridgeEndpoint] carries the port and session
     * token; feed them to [LocalhostBridgeBootstrap.script] to hand the URL to
     * the product page.
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
     * clears in-memory session state, and clears persisted session state via
     * the core-storage backend.
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
     * pending login resolves as rejected. A no-op when no login is in progress.
     */
    fun cancelLogin() {
        inner.cancelLogin()
    }

    /**
     * Activate or replace the local signing-host session from host-held secret
     * material (raw BIP-39 entropy). Lets the host run without SSO pairing.
     */
    @Throws(HostRejection::class)
    fun activateLocalSession(secret: ByteArray, liteUsername: String? = null) {
        inner.activateLocalSession(secret, liteUsername)
    }

    /**
     * Read a stored permission authorization status without prompting.
     * [request] is a SCALE-encoded `PermissionAuthorizationRequest`.
     */
    @Throws(HostRejection::class)
    fun permissionAuthorizationStatus(request: ByteArray): NativePermissionAuthorizationStatus =
        inner.permissionAuthorizationStatus(request)

    /**
     * Update a stored permission authorization status. Passing `NotDetermined`
     * clears the stored value so the next product request prompts again.
     */
    @Throws(HostRejection::class)
    fun setPermissionAuthorizationStatus(
        request: ByteArray,
        status: NativePermissionAuthorizationStatus,
    ) {
        inner.setPermissionAuthorizationStatus(request, status)
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

# TrUAPI Android host adapter

*Thin Kotlin shell over the Rust TrUAPI core (UniFFI). Wire decoding, request routing, and subscription lifecycle stay in the Rust core; products connect through the localhost WebSocket bridge.*

This directory is an Android library module: include it from a parent project's `settings.gradle.kts` (e.g. `include(":truapi-android"); project(":truapi-android").projectDir = file("vendor/truapi/android")`). It does not ship with its own Gradle wrapper or root settings — pulling it into a consuming project supplies those.

## What this package is for

The public surface lives in [`src/main/kotlin/io/parity/truapi/TrUAPIHost.kt`](src/main/kotlin/io/parity/truapi/TrUAPIHost.kt):

- `HostBridge` - callback bundle the embedding app implements. Split into device permissions, remote permissions, navigation, push, feature support, and scoped storage.
- `HostStorage` - simple read/write/clear interface the host backs with its own persistence.
- `TrUAPIHostCore` - owning wrapper around the UniFFI-generated `NativeTrUApiCore`. Holds the bridge alive for the lifetime of the core, exposes session controls and the localhost WebSocket bridge.

The generated UniFFI bindings live under `src/main/kotlin/generated/uniffi/truapi_server/`. They are committed (they're large and consumers should not need a Rust toolchain).

## Architecture

```text
product app in WebView
  Uint8Array frames via @parity/truapi createWebSocketProvider
           |
           v   ws://127.0.0.1:<port>/?t=<token>
TrUAPIHostCore.startWsBridge()
  → libtruapi_server.so (tokio WS server)
  → Rust dispatcher
```

The product running in the `WebView` opens a `WebSocket` to the localhost port + token returned by `startWsBridge`. From there the Rust core handles the wire protocol directly. Outbound responses and host-side capability callbacks (`navigateTo`, `pushNotification`, `devicePermission`, `remotePermission`, `featureSupported`, `storage`) reach the embedder through `HostBridge`.

## Permissions split

The core's `Permissions` platform trait has two methods, and so does the bridge:

- `devicePermission(request)` - OS-scoped grants (camera, mic, location, push). `request` is a SCALE-encoded `v01::HostDevicePermissionRequest`.
- `remotePermission(request)` - per-product capability bundles. `request` is a SCALE-encoded `v01::RemotePermissionRequest`.

Both return a `Boolean` granted flag. SCALE decoding for the UI prompt is done by the `@parity/truapi` JS client (or any consumer that links the protocol crate's types directly).

## Example

```kt
import android.webkit.WebView
import io.parity.truapi.HostBridge
import io.parity.truapi.HostStorage
import io.parity.truapi.TrUAPIHostCore
import uniffi.truapi_server.HostNavigateRejection
import uniffi.truapi_server.HostRejection

class MyStorage : HostStorage {
    private val map = mutableMapOf<String, ByteArray>()
    override fun read(key: String) = map[key]
    override fun write(key: String, value: ByteArray) { map[key] = value }
    override fun clear(key: String) { map.remove(key) }
}

class MyBridge : HostBridge {
    override val storage = MyStorage()
    override fun onCoreResponse(frame: ByteArray) { /* not used in WS-bridge mode */ }
    override fun navigateTo(url: String) { /* open in browser */ }
    override fun pushNotification(payload: ByteArray) { /* show notification */ }
    override fun devicePermission(request: ByteArray): Boolean = TODO("prompt user")
    override fun remotePermission(request: ByteArray): Boolean = TODO("prompt user")
    override fun featureSupported(request: ByteArray): Boolean = false
}

val core = TrUAPIHostCore(MyBridge())
val endpoint = core.startWsBridge()
val wsUrl = "ws://127.0.0.1:${endpoint.port.toInt()}/?t=${endpoint.token}"

// Inject `wsUrl` into the product page (e.g. as a query string or via an
// initial WKUserScript). Product JS uses `@parity/truapi`'s
// `createWebSocketProvider(wsUrl)` to open the wire.
val webView: WebView = existingWebView
webView.loadUrl("https://your-product.example/?truapi=${java.net.URLEncoder.encode(wsUrl, "UTF-8")}")
```

## Loading the cdylib

JNA looks for `libtruapi_server.so` in the standard `jniLibs` paths. Bundle the per-ABI builds under:

```
src/main/jniLibs/arm64-v8a/libtruapi_server.so
src/main/jniLibs/armeabi-v7a/libtruapi_server.so
src/main/jniLibs/x86_64/libtruapi_server.so
```

Build the cdylib with the `ws-bridge` feature so `startWsBridge` is functional:

```
cargo build -p truapi-server --release --features ws-bridge --target <android-target>
```

## Regenerating the bindings

The committed bindings under `src/main/kotlin/generated/uniffi/` are produced from the workspace `uniffi-bindgen-cli`:

```bash
cargo build -p truapi-server --release --features ws-bridge
cargo run -p uniffi-bindgen-cli -- generate \
  --library target/release/libtruapi_server.so \
  --language kotlin \
  --out-dir android/src/main/kotlin/generated
```

Or run `make uniffi` from the repo root.

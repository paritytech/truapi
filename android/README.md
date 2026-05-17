# TrUAPI Android host adapter

*Thin Kotlin shell over the Rust TrUAPI core (UniFFI) plus an Android `WebView` byte transport. Wire decoding, request routing, and subscription lifecycle stay in the Rust core.*

This directory is an Android library module: include it from a parent project's `settings.gradle.kts` (e.g. `include(":truapi-android"); project(":truapi-android").projectDir = file("vendor/truapi/android")`). It does not ship with its own Gradle wrapper or root settings — pulling it into a consuming project supplies those.

## What this package is for

The public surface lives in [`src/main/kotlin/io/parity/truapi/TrUAPIHost.kt`](src/main/kotlin/io/parity/truapi/TrUAPIHost.kt):

- `HostBridge` - callback bundle the embedding app implements. Split into device permissions, remote permissions, navigation, push, feature support, and scoped storage.
- `TrUAPIHostCore` - owning wrapper around the UniFFI-generated `NativeTrUApiCore`. Implements `CoreInbound`, owns the bridge lifetime, exposes session and WS bridge controls.
- `WebViewTransport` - base64-over-`JavascriptInterface` byte pipe between a `WebView` and any `CoreInbound`. Injects a `window.trUApi` shim that matches the JS host adapter shape.
- `bootstrapScript` - the JS shim, exposed so apps can inject it through their own WebView bootstrap path.

The generated UniFFI bindings live under `src/main/kotlin/generated/uniffi/truapi_server/`. They are committed (they're large and consumers should not need a Rust toolchain).

## Architecture

```text
product app in WebView
  Uint8Array frames via window.trUApi
           |
           v
WebViewTransport
  base64 over Android JS bridge
           |
           v
TrUAPIHostCore (CoreInbound)
  → uniffi → libtruapi_server.so
```

Inbound flow:

1. Product JS calls `window.trUApi.postMessage(bytes)`
2. `WebViewTransport` receives base64 through `@JavascriptInterface`
3. `TrUAPIHostCore.receiveFromProduct(...)` forwards bytes into the Rust dispatcher
4. The Rust core emits a response frame; `HostBridge.onCoreResponse(...)` fires
5. The embedder typically pumps the response back through `WebViewTransport.sendToProduct(...)`, which calls `window.__trUApiReceive(...)`

The Rust core also calls `HostBridge` directly for platform capabilities: `navigateTo`, `pushNotification`, `devicePermission`, `remotePermission`, `featureSupported`, and the `storage` slot.

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
import io.parity.truapi.WebViewTransport
import uniffi.truapi_server.HostNavigateRejection
import uniffi.truapi_server.HostRejection

class MyStorage : HostStorage {
    private val map = mutableMapOf<String, ByteArray>()
    override fun read(key: String) = map[key]
    override fun write(key: String, value: ByteArray) { map[key] = value }
    override fun clear(key: String) { map.remove(key) }
}

class MyBridge(private val transport: WebViewTransport) : HostBridge {
    override val storage = MyStorage()
    override fun onCoreResponse(frame: ByteArray) = transport.sendToProduct(frame)
    override fun navigateTo(url: String) { /* open in browser */ }
    override fun pushNotification(payload: ByteArray) { /* show notification */ }
    override fun devicePermission(request: ByteArray): Boolean = TODO("prompt user")
    override fun remotePermission(request: ByteArray): Boolean = TODO("prompt user")
    override fun featureSupported(request: ByteArray): Boolean = false
}

val webView: WebView = existingWebView
lateinit var transport: WebViewTransport
val bridge = MyBridge(transport = WebViewTransport(webView, core = object : io.parity.truapi.CoreInbound {
    override fun receiveFromProduct(frame: ByteArray) = core.receiveFromProduct(frame)
}).also { transport = it })
val core = TrUAPIHostCore(bridge)
transport.attach()
```

(In practice, build the `TrUAPIHostCore` first, hand it to a `WebViewTransport`, and have the bridge close back over the same transport instance.)

## Loading the cdylib

JNA looks for `libtruapi_server.so` in the standard `jniLibs` paths. Bundle the per-ABI builds under:

```
src/main/jniLibs/arm64-v8a/libtruapi_server.so
src/main/jniLibs/armeabi-v7a/libtruapi_server.so
src/main/jniLibs/x86_64/libtruapi_server.so
```

Build the cdylib with the `ws-bridge` feature if you want `startWsBridge` to be functional:

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

# TrUAPI Android host adapter

*Kotlin wrapper around the TrUAPI Rust core (UniFFI). Wire decoding, request routing, and subscription lifecycle stay in the Rust core; products connect through the localhost WebSocket bridge.*

Distributed as a Maven artifact built on demand from git tags by [JitPack](https://jitpack.io/), no Maven Central account required on either side.

## Consume

Add the JitPack Maven repository and the artifact to your app's Gradle build:

```kotlin
// settings.gradle.kts
dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
        maven { url = uri("https://jitpack.io") }
    }
}
```

```kotlin
// app/build.gradle.kts
dependencies {
    implementation("com.github.paritytech.truapi:truapi-host:0.1.0")
}
```

JitPack fetches the tag `0.1.0` from `paritytech/truapi`, runs `gradle :truapi-host:publishReleasePublicationToMavenLocal` against it (driven by `jitpack.yml` at the repo root), and serves the resulting AAR + POM + sources jar. First fetch takes ~1 minute while JitPack builds; subsequent consumers hit the cache.

The artifact bundles the Kotlin host adapter (`io.parity.truapi.*`) and the generated UniFFI bindings (`uniffi.truapi_server.*`). It does **not** bundle the native `libtruapi_server.so` cdylib, integrators build that per Android ABI and drop it into their app's `src/main/jniLibs/<abi>/` (see "Linking the cdylib" below).

### Compatibility

- **minSdk**: 29 (Android 10). Aligns with the polkadot-app-android-v2 floor.
- **AGP**: built with 8.5.2; AGP 8.5+ consumers are fine. AAR is forward-compatible with newer AGPs.
- **Kotlin**: built with 1.9.24. Newer Kotlin compilers (2.x) read 1.9 metadata fine.
- **Transitive dependency**: the AAR pulls `net.java.dev.jna:jna:5.14.0` (UniFFI's runtime). Consumers that don't already use JNA will see ~1.5MB added to their app.

## Public surface

The public surface lives in [`src/main/kotlin/io/parity/truapi/TrUAPIHost.kt`](src/main/kotlin/io/parity/truapi/TrUAPIHost.kt):

- `HostBridge` - callback bundle the embedding app implements. Splits device permissions, remote permissions, navigation, push, feature support, and scoped storage.
- `HostStorage` - read/write/clear interface the host backs with its own persistence.
- `TrUAPIHostCore` - owning wrapper around the UniFFI-generated `NativeTrUApiCore`. Holds the bridge alive for the lifetime of the core, exposes session controls and the localhost WebSocket bridge.

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

> **Threading:** when the WS bridge is running, the Rust core invokes every
> `HostBridge` callback on the dedicated `truapi-ws-bridge` worker thread, never
> the UI thread. Marshal any UI work (navigation, prompts, notifications,
> touching the `WebView`) onto the main thread with
> `Handler(Looper.getMainLooper())` or a `Dispatchers.Main` `CoroutineScope`.
> Permission callbacks return synchronously, so block the worker thread (e.g. a
> `CountDownLatch`) until the main-thread prompt resolves.

```kt
import android.os.Handler
import android.os.Looper
import android.webkit.WebView
import io.parity.truapi.HostBridge
import io.parity.truapi.HostStorage
import io.parity.truapi.TrUAPIHostCore
import java.util.concurrent.CountDownLatch

class MyStorage : HostStorage {
    private val map = mutableMapOf<String, ByteArray>()
    override fun read(key: String) = map[key]
    override fun write(key: String, value: ByteArray) { map[key] = value }
    override fun clear(key: String) { map.remove(key) }
}

class MyBridge(private val webView: WebView) : HostBridge {
    private val main = Handler(Looper.getMainLooper())

    override val storage = MyStorage()

    override fun navigateTo(url: String) {
        main.post { /* startActivity(Intent(ACTION_VIEW, Uri.parse(url))) */ }
    }

    override fun pushNotification(payload: ByteArray) {
        main.post { /* show notification */ }
    }

    override fun devicePermission(request: ByteArray): Boolean {
        // Called on the worker thread; prompt on the main thread and wait.
        val latch = CountDownLatch(1)
        var granted = false
        main.post { /* show prompt, set granted, then */ latch.countDown() }
        latch.await()
        return granted
    }

    override fun remotePermission(request: ByteArray): Boolean = TODO("prompt user")
    override fun featureSupported(request: ByteArray): Boolean = false
}

val webView: WebView = existingWebView
val core = TrUAPIHostCore(MyBridge(webView))
val endpoint = core.startWsBridge()
val wsUrl = "ws://127.0.0.1:${endpoint.port.toInt()}/?t=${endpoint.token}"

// Inject `wsUrl` into the product page; product JS calls
// `@parity/truapi`'s `createWebSocketProvider(wsUrl)` to open the wire.
webView.loadUrl("https://your-product.example/?truapi=${java.net.URLEncoder.encode(wsUrl, "UTF-8")}")
```

## Linking the cdylib

The native runtime ships separately. JNA looks for `libtruapi_server.so` in the standard `jniLibs` paths; bundle the per-ABI builds under:

```
src/main/jniLibs/arm64-v8a/libtruapi_server.so
src/main/jniLibs/armeabi-v7a/libtruapi_server.so
src/main/jniLibs/x86_64/libtruapi_server.so
```

Cross-build the cdylib for each Android ABI from the truapi monorepo. Two options, pick whichever fits the host app's existing toolchain:

**Option A: `mozilla-rust-android-gradle` plugin.** Recommended if the host app already uses it (polkadot-app-android-v2 does, for `bandersnatch-crypto`). Vendor `paritytech/truapi` as a git submodule, add a small Gradle module that points the plugin at `rust/crates/truapi-server`:

```kotlin
// app/build.gradle.kts (or a dedicated :truapi-cdylib module)
plugins {
    alias(libs.plugins.mozilla.rust.android)
}

cargo {
    module = "<path>/truapi/rust/crates/truapi-server"
    libname = "truapi_server"
    targets = listOf("arm64", "arm", "x86_64")
    profile = "release"
    features { defaultAnd(arrayOf("ws-bridge")) }
}

tasks.matching { it.name.matches("merge.*JniLibFolders".toRegex()) }.configureEach {
    inputs.dir(layout.buildDirectory.dir("rustJniLibs/android"))
    dependsOn("cargoBuild")
}
```

**Option B: `cargo-ndk` from the command line.** Standalone, no Gradle plugin required:

```bash
cargo install cargo-ndk
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 \
  -o app/src/main/jniLibs \
  build --release -p truapi-server --features ws-bridge
```

Both options require the Android NDK installed and the matching Rust targets (`rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android`).

Pre-built per-ABI `.so` files bundled inside the AAR are tracked as a follow-up so consumers eventually don't need a Rust toolchain at all.

## Maintainers: cutting a release

JitPack builds on demand from any git tag in `paritytech/truapi`, so a release is just:

1. Bump `publicationVersion` in `android/truapi-host/build.gradle.kts`.
2. Commit. Open a PR. Merge.
3. Tag the merge commit with the version: `git tag truapi-host-android@0.1.0 && git push origin truapi-host-android@0.1.0`.

That's the entire release flow, the iOS Swift Package follows the same pattern. The first consumer to pull the tag will trigger JitPack to build the artifact; subsequent fetches hit the cache.

For local development, publish into the dev `~/.m2`:

```bash
gradle :truapi-host:publishReleasePublicationToMavenLocal
# or
make android-publish-local
```

The artifact lands under `~/.m2/repository/io/parity/truapi-host-android/<version>/`. Consumers pointing at `mavenLocal()` can resolve it via `io.parity:truapi-host-android:<version>`. These local coordinates differ from the JitPack consumer coordinate (`com.github.paritytech.truapi:truapi-host:<tag>`): JitPack derives the group and artifactId from the repo and Gradle subproject, overriding the `io.parity:truapi-host-android` coordinates set in `build.gradle.kts`.

## Regenerating the UniFFI bindings

The committed Kotlin bindings under `src/main/kotlin/generated/uniffi/` are produced from the workspace `uniffi-bindgen-cli`:

```bash
cargo build -p truapi-server --release --features ws-bridge
cargo run -p uniffi-bindgen-cli -- generate \
  --library target/release/libtruapi_server.so \
  --language kotlin \
  --out-dir android/truapi-host/src/main/kotlin/generated
```

Or run `make uniffi` from the repo root.

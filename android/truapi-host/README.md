# TrUAPI Android host adapter

*Kotlin wrapper around the TrUAPI Rust core (UniFFI). Wire decoding, request routing, and subscription lifecycle stay in the Rust core; products connect through the localhost WebSocket bridge.*

> **Status:** the JitPack distribution described below is the intended packaging but is **not yet wired up** — there is no `jitpack.yml` at the repo root, so the "add the JitPack repo and depend on the tag" flow does not work today. Until it is added, integrate locally with `make android-publish-local` + `mavenLocal()`, or build the module directly. The rest of this doc describes the target design.

Intended distribution: a Maven artifact built on demand from git tags by [JitPack](https://jitpack.io/), no Maven Central account required on either side.

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

JitPack fetches the tag `0.1.0` from `paritytech/truapi`, runs `make android-publish-local` against it (driven by `jitpack.yml` at the repo root, including UniFFI binding generation), and serves the resulting AAR + POM + sources jar. First fetch takes ~1 minute while JitPack builds; subsequent consumers hit the cache.

The artifact bundles the Kotlin host adapter (`io.parity.truapi.*`) and the generated UniFFI bindings (`uniffi.truapi_server.*`). It does **not** bundle the native `libtruapi_server.so` cdylib, integrators build that per Android ABI and drop it into their app's `src/main/jniLibs/<abi>/` (see "Linking the cdylib" below).

The consuming app must declare `android.permission.INTERNET` — the localhost WebSocket bridge binds a `127.0.0.1` TCP socket, which requires it even for loopback.

### Compatibility

- **minSdk**: 29 (Android 10). Aligns with the polkadot-app-android-v2 floor.
- **AGP**: built with 8.5.2; AGP 8.5+ consumers are fine. AAR is forward-compatible with newer AGPs.
- **Kotlin**: built with 1.9.24. Newer Kotlin compilers (2.x) read 1.9 metadata fine.
- **Transitive dependency**: the AAR pulls `net.java.dev.jna:jna:5.14.0` (UniFFI's runtime). Consumers that don't already use JNA will see ~1.5MB added to their app.

## Public surface

The public surface lives in [`src/main/kotlin/io/parity/truapi/TrUAPIHost.kt`](src/main/kotlin/io/parity/truapi/TrUAPIHost.kt):

- `HostBridge` - callback bundle the embedding app implements. Splits device permissions, remote permissions, navigation, push, feature support, a single `confirmUserAction`, and both storage backends.
- `HostStorage` - product-scoped read/write/clear interface the host backs with its own persistence.
- `HostCoreStorage` - core-owned read/write/clear interface for auth session, pairing identity, and persisted permission decisions (`key` is a SCALE-encoded `CoreStorageKey`).
- `TrUAPIHostCore` - owning wrapper around the UniFFI-generated `NativeTrUApiCore`. Holds the bridge alive for the lifetime of the core and exposes the localhost WebSocket bridge, core-owned disconnect, local-session activation, permission-authorization status, and native change notifications for session storage, theme, and preimage updates.
- `LocalhostBridgeBootstrap` - JS snippet that publishes the WS bridge endpoint (`window.__truapi_localhost`) to the product page so it can dial back in.

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

The product running in the `WebView` opens a `WebSocket` to the localhost port + token returned by `startWsBridge`. From there the Rust core handles the wire protocol directly. Outbound responses and host-side capability callbacks (`navigateTo`, `pushNotification`, `cancelNotification`, `devicePermission`, `remotePermission`, `authStateChanged`, core storage, chain JSON-RPC, `confirmUserAction`, preimage lookup, theme, `featureSupported`, `storage`) reach the embedder through `HostBridge`. Bulletin preimage build/sign/submit now happens inside the core, so the host only serves `lookupPreimage`.

## Permissions split

The core's `Permissions` platform trait has two methods, and so does the bridge:

- `devicePermission(request)` - OS-scoped grants (camera, mic, location, push). `request` is a SCALE-encoded `v01::HostDevicePermissionRequest`.
- `remotePermission(request)` - per-product capability bundles. `request` is a SCALE-encoded `v01::RemotePermissionRequest`.

Both return a `Boolean` granted flag. SCALE decoding for the UI prompt is done by the `@parity/truapi` JS client (or any consumer that links the protocol crate's types directly).

## Example

> **Threading:** the Rust core invokes every `HostBridge` callback on a
> background thread it owns, never the UI thread. Marshal any UI work
> (navigation, prompts, notifications, touching the `WebView`) onto the main
> thread with `Handler(Looper.getMainLooper())` or a `Dispatchers.Main`
> `CoroutineScope`. UI-decision callbacks (`navigateTo`, `devicePermission`,
> `remotePermission`, `confirmUserAction`) each run on their own blocking-pool
> thread, so it is safe to block the calling thread (e.g. with a
> `CountDownLatch`) until the main-thread prompt resolves; other TrUAPI traffic
> keeps flowing while you wait. The remaining callbacks (auth state, storage,
> core storage, chain, feature, theme, preimage lookups) run inline on the
> dispatcher thread and must return promptly without blocking.

```kt
import android.os.Handler
import android.os.Looper
import android.webkit.WebView
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import io.parity.truapi.HostBridge
import io.parity.truapi.HostCoreStorage
import io.parity.truapi.HostStorage
import io.parity.truapi.LocalhostBridgeBootstrap
import io.parity.truapi.PairingDeeplinkScheme
import io.parity.truapi.RuntimeConfig
import io.parity.truapi.TrUAPIHostCore
import uniffi.truapi_server.AuthState
import uniffi.truapi_server.HostTheme
import java.util.concurrent.CountDownLatch

class MyStorage : HostStorage {
    private val map = mutableMapOf<String, ByteArray>()
    override fun read(key: String) = map[key]
    override fun write(key: String, value: ByteArray) { map[key] = value }
    override fun clear(key: String) { map.remove(key) }
}

// Core-owned storage: keyed by SCALE-encoded CoreStorageKey bytes. Back it with
// real persistence (e.g. EncryptedSharedPreferences); an in-memory map is shown
// for brevity.
class MyCoreStorage : HostCoreStorage {
    private val map = HashMap<String, ByteArray>()
    private fun k(key: ByteArray) = key.joinToString("") { "%02x".format(it) }
    override fun read(key: ByteArray) = map[k(key)]
    override fun write(key: ByteArray, value: ByteArray) { map[k(key)] = value }
    override fun clear(key: ByteArray) { map.remove(k(key)) }
}

class MyBridge(private val webView: WebView) : HostBridge {
    private val main = Handler(Looper.getMainLooper())

    override val storage = MyStorage()
    override val coreStorage = MyCoreStorage()

    override fun navigateTo(url: String) {
        main.post { /* startActivity(Intent(ACTION_VIEW, Uri.parse(url))) */ }
    }

    override fun pushNotification(payload: ByteArray): UInt {
        val id = 1u
        main.post { /* show notification */ }
        return id
    }

    override fun cancelNotification(id: UInt) {
        main.post { /* cancel notification */ }
    }

    override fun devicePermission(request: ByteArray): Boolean {
        // Called on a blocking-pool thread; prompt on the main thread and
        // wait. Blocking here does not stall other TrUAPI traffic.
        val latch = CountDownLatch(1)
        var granted = false
        main.post { /* show prompt, set granted, then */ latch.countDown() }
        latch.await()
        return granted
    }

    override fun remotePermission(request: ByteArray): Boolean = false
    override fun featureSupported(request: ByteArray): Boolean = false

    // Core-owned auth state stream: render AuthState.Pairing as the pairing
    // QR sheet, connected/disconnected as the account badge, and login-failed
    // as a retryable error. When the user closes the pairing sheet, report it
    // with `core.cancelLogin()`.
    override fun authStateChanged(state: AuthState) {
        main.post { /* render the state */ }
    }

    override fun chainConnect(genesisHash: ByteArray): UInt? {
        val id = 1u
        main.post { /* open JSON-RPC connection, forward responses via core.notifyChainResponse */ }
        return id
    }

    override fun chainSend(connectionId: UInt, request: String) {
        /* send JSON-RPC request on the host connection */
    }

    override fun chainClose(connectionId: UInt) {
        /* close host connection */
    }

    // One confirmation callback for every reviewed core action. Decode
    // `review` (SCALE `UserConfirmationReview`) with the @parity/truapi JS
    // client to pick the prompt (sign payload / raw / create tx / alias /
    // resource allocation / preimage submit).
    override fun confirmUserAction(review: ByteArray): Boolean {
        val latch = CountDownLatch(1)
        var approved = false
        main.post { /* show prompt, set approved, then */ latch.countDown() }
        latch.await()
        return approved
    }
}

val webView: WebView = existingWebView
val runtimeConfig = RuntimeConfig(
    productId = "my-product.dot",
    hostName = "My Host",
    hostIcon = "https://host.example/icon.png",
    peopleChainGenesisHash = ByteArray(32),
    bulletinChainGenesisHash = ByteArray(32),
    // Optional: activate a local signing session from host-held BIP-39 entropy
    // (no SSO pairing). Omit for the QR pairing flow.
    localSessionSecret = null,
    pairingDeeplinkScheme = PairingDeeplinkScheme.POLKADOT_APP,
)
val core = TrUAPIHostCore(MyBridge(webView), runtimeConfig)
val endpoint = core.startWsBridge()

// Call these from host/platform observers so native subscriptions see updates
// after their immediate current item.
core.notifySessionStoreChanged()
core.notifyThemeChanged(HostTheme.DARK)
core.notifyPreimageChanged(preimageKey, preimageBytesOrNull)
core.notifyChainResponse(chainConnectionId, jsonRpcResponse)
core.notifyChainClosed(chainConnectionId)

// Publish the bridge endpoint to the product page. Install the bootstrap as a
// DOCUMENT-START script so it runs in the destination document before the page
// scripts — `evaluateJavascript` runs in the CURRENT document, which the
// following `loadUrl` replaces, so the product would lose the endpoint. Scope
// it to the product origin. The page reads `window.__truapi_localhost.url` and
// passes it to `@parity/truapi`'s `createWebSocketProvider`.
val bootstrap = LocalhostBridgeBootstrap.script(endpoint.port, endpoint.token)
main.post {
    val productUrl = "https://your-product.example/"
    if (WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
        WebViewCompat.addDocumentStartJavaScript(
            webView,
            bootstrap,
            setOf("https://your-product.example"), // origin allowlist
        )
    }
    webView.loadUrl(productUrl)
}

// On logout:
core.disconnect()
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

The ignored Kotlin bindings under `src/main/kotlin/generated/uniffi/` are produced from the workspace `uniffi-bindgen-cli`. Regenerate them before building or publishing the Android host package:

```bash
make uniffi-kotlin
```

`make uniffi-kotlin` builds the host cdylib with the `codegen` profile and runs
the generator. The `codegen` profile is required because uniffi-bindgen scans
the cdylib's exported metadata symbols, which the `release` profile strips — a
plain `--release` build produces a stripped library and no bindings. (`make
uniffi` regenerates the Swift bindings; use `make uniffi-kotlin` for Android.)

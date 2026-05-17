// TrUAPI Android host adapter.
//
// Wraps the UniFFI-generated bindings in `src/main/kotlin/generated/uniffi/`
// behind a thin Kotlin API. Products running in a `WebView` connect to the
// Rust core through its localhost WebSocket bridge (see
// `TrUAPIHostCore.startWsBridge`); the Rust core (compiled to
// `libtruapi_server.so`) handles wire decoding, routing, subscription
// lifecycle, and host capability dispatch.

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "io.parity.truapi"
    compileSdk = 34

    defaultConfig {
        minSdk = 26
    }

    sourceSets {
        getByName("main") {
            java.srcDirs("src/main/kotlin")
            manifest.srcFile("src/main/AndroidManifest.xml")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    // UniFFI Kotlin bindings use JNA for FFI.
    implementation("net.java.dev.jna:jna:5.14.0@aar")
    // The generated callback adapter wraps user-supplied lambdas; coroutines
    // are not required by the bindings themselves but are commonly used by
    // consumers when dispatching on a background scope.
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.1")
}

// TrUAPI Android host adapter.
//
// Publishes `io.parity:truapi-host-android` to Maven. Products running in a
// `WebView` connect to the Rust core via its localhost WebSocket bridge
// (`TrUAPIHostCore.startWsBridge`); the Rust core (compiled to
// `libtruapi_server.so`) handles wire decoding, routing, subscription
// lifecycle, and host capability dispatch.

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("maven-publish")
}

android {
    namespace = "io.parity.truapi"
    compileSdk = 34

    defaultConfig {
        // minSdk 29 matches the polkadot-app-android-v2 floor; raise here
        // first and bump consumers' floors if we ever depend on a newer API.
        minSdk = 29
        consumerProguardFiles("consumer-rules.pro")
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

    publishing {
        singleVariant("release") {
            withSourcesJar()
            withJavadocJar()
        }
    }
}

dependencies {
    // UniFFI Kotlin bindings use JNA for FFI.
    api("net.java.dev.jna:jna:5.14.0@aar")
}

// Coordinates for the local Maven publication (`publishToMavenLocal`).
// Distribution is via JitPack: a git tag drives `jitpack.yml`, and JitPack
// derives the consumer coordinates from the repo + subproject as
// `com.github.paritytech.truapi:truapi-host:<tag>`, overriding the group and
// artifactId below. These fields only matter for local testing.
val publicationGroup = "io.parity"
val publicationArtifact = "truapi-host-android"
val publicationVersion = "0.1.0"

group = publicationGroup
version = publicationVersion

publishing {
    publications {
        register<MavenPublication>("release") {
            groupId = publicationGroup
            artifactId = publicationArtifact
            version = publicationVersion

            afterEvaluate {
                from(components["release"])
            }

            pom {
                name.set("TrUAPI Android host adapter")
                description.set(
                    "Kotlin wrapper around the TrUAPI Rust core (UniFFI). " +
                        "Hosts integrating a `WebView`-based product link the " +
                        "`libtruapi_server` cdylib and route product traffic " +
                        "through the localhost WebSocket bridge."
                )
                url.set("https://github.com/paritytech/truapi")
                licenses {
                    license {
                        name.set("MIT")
                        url.set("https://github.com/paritytech/truapi/blob/main/LICENSE")
                    }
                }
                scm {
                    connection.set("scm:git:https://github.com/paritytech/truapi.git")
                    developerConnection.set("scm:git:ssh://git@github.com/paritytech/truapi.git")
                    url.set("https://github.com/paritytech/truapi")
                }
                developers {
                    developer {
                        name.set("Parity Technologies")
                        email.set("admin@parity.io")
                        organization.set("Parity Technologies")
                        organizationUrl.set("https://parity.io")
                    }
                }
            }
        }
    }

    repositories {
        // Maven Local for `gradle publishToMavenLocal` during development
        // and for JitPack's build environment (see `jitpack.yml`).
        // Consumers fetch the published artifact via JitPack at
        // `com.github.paritytech.truapi:truapi-host:<tag>` after the
        // repo is tagged.
        mavenLocal()
    }
}

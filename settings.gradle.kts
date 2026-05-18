// Gradle workspace root for the Android host library.
//
// The library module under `android/` is published as a Maven artifact
// (`io.parity:truapi-host-android`). Run `./gradlew :truapi-android:publish*`
// from this directory; consumers integrate via Maven coordinates, not by
// including this module from their own settings.gradle.kts.
//
// The rest of the repo (Rust crates, JS packages, iOS Swift Package) does
// not use Gradle.

pluginManagement {
    repositories {
        gradlePluginPortal()
        google()
        mavenCentral()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "truapi"

include(":truapi-android")
project(":truapi-android").projectDir = file("android")

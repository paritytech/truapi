// Gradle workspace root for the Android host packages.
//
// Library modules live one level down at `android/<package>/`. Run
// `gradle :<package>:publish*` from this directory; consumers integrate
// via Maven coordinates, not by including a module from their own
// settings.gradle.kts.
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

include(":truapi-host")
project(":truapi-host").projectDir = file("android/truapi-host")

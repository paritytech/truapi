// Standalone settings so this directory can be assembled in isolation.
// When consumed from a host app, prefer including it via the host's
// `settings.gradle.kts` (e.g. `include(":host-libs:android")`).

pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "truapi-host-android"

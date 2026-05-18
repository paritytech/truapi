// Empty root project. All build logic lives in the `:truapi-android` module
// under `android/`. This file exists so `./gradlew` finds a build script at
// the workspace root.

plugins {
    id("com.android.library") version "8.5.2" apply false
    id("org.jetbrains.kotlin.android") version "1.9.24" apply false
}

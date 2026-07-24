# ProGuard / R8 rules applied to consumers of `io.parity:truapi-host-android`.
#
# JNA reflects into our generated UniFFI types at runtime, so the bindings
# package and the public Kotlin surface must survive shrinking.

-keep class uniffi.truapi_server.** { *; }
-keep class io.parity.truapi.** { *; }

# JNA itself.
-keep class com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.** { *; }
